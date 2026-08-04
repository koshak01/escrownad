// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "./IERC20.sol";

/// @title EscrowNad proof-escrow lock (Monad) — **USDC first-class**
/// @notice Buyer deposits **USDC (ERC-20)** after `approve`; observer releases/refunds USDC.
/// @dev Settlement asset = USDC. Native MON is gas only (not deal funds).
///      Constructor takes `usdc` token + `observer` EOA (Rust service, v1 single key).
///      Dev: deploy MockUSDC then EscrowLock(mockUsdc, observer).
contract EscrowLock {
    address public owner;
    address public observer;
    IERC20 public immutable usdc;

    enum State {
        None,
        Funded,
        Released,
        Refunded
    }

    struct Deal {
        address seller;
        address buyer;
        uint256 amount; // USDC base units (6 decimals)
        uint64 deadline; // unix sec; 0 = no on-chain timeout hatch
        State state;
        bytes32 conditionHash;
    }

    mapping(bytes32 => Deal) public deals;

    event Funded(
        bytes32 indexed dealId,
        address indexed buyer,
        address indexed seller,
        uint256 amount,
        uint64 deadline
    );
    event Released(bytes32 indexed dealId, address indexed seller, uint256 amount, bytes32 ripeKey);
    event Refunded(bytes32 indexed dealId, address indexed buyer, uint256 amount);

    error NotOwner();
    error NotObserver();
    error BadState();
    error BadValue();
    error ZeroAddress();
    error TransferFailed();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyObserver() {
        if (msg.sender != observer) revert NotObserver();
        _;
    }

    constructor(address usdc_, address observer_) {
        if (usdc_ == address(0) || observer_ == address(0)) revert ZeroAddress();
        owner = msg.sender;
        usdc = IERC20(usdc_);
        observer = observer_;
    }

    function setObserver(address observer_) external onlyOwner {
        if (observer_ == address(0)) revert ZeroAddress();
        observer = observer_;
    }

    /// @notice Buyer funds deal in USDC. Caller must `usdc.approve(this, amount)` first.
    function fund(
        bytes32 dealId,
        address seller,
        uint256 amount,
        uint64 deadline,
        bytes32 conditionHash
    ) external {
        if (seller == address(0)) revert ZeroAddress();
        if (amount == 0) revert BadValue();
        Deal storage d = deals[dealId];
        if (d.state != State.None) revert BadState();

        d.seller = seller;
        d.buyer = msg.sender;
        d.amount = amount;
        d.deadline = deadline;
        d.state = State.Funded;
        d.conditionHash = conditionHash;

        bool ok = usdc.transferFrom(msg.sender, address(this), amount);
        if (!ok) revert TransferFailed();

        emit Funded(dealId, msg.sender, seller, amount, deadline);
    }

    /// @notice Observer releases USDC to seller after RIPE proof.
    function release(bytes32 dealId, bytes32 ripeKey) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Released;
        uint256 amount = d.amount;
        address seller = d.seller;
        bool ok = usdc.transfer(seller, amount);
        if (!ok) revert TransferFailed();
        emit Released(dealId, seller, amount, ripeKey);
    }

    /// @notice Observer refunds USDC to buyer (no fact / grey).
    function refund(bytes32 dealId) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount;
        address buyer = d.buyer;
        bool ok = usdc.transfer(buyer, amount);
        if (!ok) revert TransferFailed();
        emit Refunded(dealId, buyer, amount);
    }

    /// @notice Anyone can refund USDC after deadline if observer is offline.
    function refundAfterDeadline(bytes32 dealId) external {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        if (d.deadline == 0 || block.timestamp < d.deadline) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount;
        address buyer = d.buyer;
        bool ok = usdc.transfer(buyer, amount);
        if (!ok) revert TransferFailed();
        emit Refunded(dealId, buyer, amount);
    }
}
