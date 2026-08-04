// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title EscrowNad proof-escrow lock (Monad)
/// @notice Buyer deposits native MON (`payable`); only `observer` can release/refund.
/// @dev v1 settlement asset = native MON. USDC/ERC-20 = later path.
///      Deploy on Monad testnet. Observer = Rust service EOA (single key).
contract EscrowLock {
    address public owner;
    address public observer;

    enum State {
        None,
        Funded,
        Released,
        Refunded
    }

    struct Deal {
        address seller;
        address buyer;
        uint256 amount;
        uint64 deadline; // unix seconds; 0 = no timeout enforced on-chain
        State state;
        bytes32 conditionHash; // optional: hash of RIPE match key / deal id
    }

    mapping(bytes32 => Deal) public deals;

    event Funded(bytes32 indexed dealId, address indexed buyer, address indexed seller, uint256 amount, uint64 deadline);
    event Released(bytes32 indexed dealId, address indexed seller, uint256 amount, bytes32 ripeKey);
    event Refunded(bytes32 indexed dealId, address indexed buyer, uint256 amount);

    error NotOwner();
    error NotObserver();
    error BadState();
    error BadValue();
    error ZeroAddress();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyObserver() {
        if (msg.sender != observer) revert NotObserver();
        _;
    }

    constructor(address observer_) {
        if (observer_ == address(0)) revert ZeroAddress();
        owner = msg.sender;
        observer = observer_;
    }

    function setObserver(address observer_) external onlyOwner {
        if (observer_ == address(0)) revert ZeroAddress();
        observer = observer_;
    }

    /// @notice Buyer funds the lock for a deal id (bytes32, e.g. keccak256 of del_hash).
    function fund(bytes32 dealId, address seller, uint64 deadline, bytes32 conditionHash) external payable {
        if (seller == address(0)) revert ZeroAddress();
        if (msg.value == 0) revert BadValue();
        Deal storage d = deals[dealId];
        if (d.state != State.None) revert BadState();
        d.seller = seller;
        d.buyer = msg.sender;
        d.amount = msg.value;
        d.deadline = deadline;
        d.state = State.Funded;
        d.conditionHash = conditionHash;
        emit Funded(dealId, msg.sender, seller, msg.value, deadline);
    }

    /// @notice Observer releases funds to seller after RIPE proof.
    function release(bytes32 dealId, bytes32 ripeKey) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Released;
        uint256 amount = d.amount;
        address seller = d.seller;
        (bool ok, ) = seller.call{value: amount}("");
        require(ok, "transfer failed");
        emit Released(dealId, seller, amount, ripeKey);
    }

    /// @notice Observer refunds buyer (timeout / no fact).
    function refund(bytes32 dealId) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount;
        address buyer = d.buyer;
        (bool ok, ) = buyer.call{value: amount}("");
        require(ok, "transfer failed");
        emit Refunded(dealId, buyer, amount);
    }

    /// @notice Anyone can refund after deadline if observer is offline (safety hatch).
    function refundAfterDeadline(bytes32 dealId) external {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        if (d.deadline == 0 || block.timestamp < d.deadline) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount;
        address buyer = d.buyer;
        (bool ok, ) = buyer.call{value: amount}("");
        require(ok, "transfer failed");
        emit Refunded(dealId, buyer, amount);
    }
}
