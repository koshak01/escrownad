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
    /// @notice Куда уходит комиссия площадки.
    address public treasury;
    IERC20 public immutable usdc;

    /// @notice Комиссия площадки, в сотых долях процента. 100 = 1%.
    /// @dev Платит ТОЛЬКО покупатель — тот, кто отдаёт деньги. Он вносит
    ///      цену плюс комиссию сверху. Владелец актива (IP-блока, товара)
    ///      получает ровно свою цену и не платит ничего.
    ///      Потолок 500 (5%) зашит в setFee — владелец не может задрать
    ///      комиссию произвольно, это защита пользователей от нас самих.
    uint16 public feeBps = 100;
    uint16 public constant MAX_FEE_BPS = 500;

    enum State {
        None,
        Funded,
        Released,
        Refunded
    }

    struct Deal {
        address seller;
        address buyer;
        uint256 amount; // цена сделки, USDC base units (6 decimals)
        uint256 fee; // комиссия площадки, внесена покупателем сверх цены
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
        uint256 fee,
        uint64 deadline
    );
    event Released(
        bytes32 indexed dealId,
        address indexed seller,
        uint256 paidToSeller,
        uint256 feeTaken,
        bytes32 ripeKey
    );
    event Refunded(bytes32 indexed dealId, address indexed buyer, uint256 amount);
    event FeeChanged(uint16 feeBps, address treasury);

    error NotOwner();
    error NotObserver();
    error BadState();
    error BadValue();
    error ZeroAddress();
    error TransferFailed();
    error FeeTooHigh();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyObserver() {
        if (msg.sender != observer) revert NotObserver();
        _;
    }

    constructor(address usdc_, address observer_, address treasury_) {
        if (usdc_ == address(0) || observer_ == address(0) || treasury_ == address(0)) {
            revert ZeroAddress();
        }
        owner = msg.sender;
        usdc = IERC20(usdc_);
        observer = observer_;
        treasury = treasury_;
    }

    function setObserver(address observer_) external onlyOwner {
        if (observer_ == address(0)) revert ZeroAddress();
        observer = observer_;
    }

    /// @notice Меняет комиссию и получателя. Потолок 5% — выше нельзя.
    /// @dev На уже профинансированные сделки НЕ влияет: комиссия посчитана
    ///      и записана в момент fund. Задним числом её изменить нельзя.
    function setFee(uint16 feeBps_, address treasury_) external onlyOwner {
        if (feeBps_ > MAX_FEE_BPS) revert FeeTooHigh();
        if (treasury_ == address(0)) revert ZeroAddress();
        feeBps = feeBps_;
        treasury = treasury_;
        emit FeeChanged(feeBps_, treasury_);
    }

    /// @notice Сколько покупатель переведёт всего: цена + комиссия площадки.
    /// @dev Фронт зовёт это перед approve, чтобы показать полную сумму.
    function quote(uint256 amount) public view returns (uint256 total, uint256 fee) {
        fee = (amount * feeBps) / 10_000;
        total = amount + fee;
    }

    /// @notice Покупатель вносит цену + свою комиссию. Перед вызовом нужен
    ///         `usdc.approve(this, total)`, где total = quote(amount).total.
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

        (uint256 total, uint256 fee) = quote(amount);

        d.seller = seller;
        d.buyer = msg.sender;
        d.amount = amount;
        // комиссию фиксируем здесь же — задним числом её изменить нельзя
        d.fee = fee;
        d.deadline = deadline;
        d.state = State.Funded;
        d.conditionHash = conditionHash;

        bool ok = usdc.transferFrom(msg.sender, address(this), total);
        if (!ok) revert TransferFailed();

        emit Funded(dealId, msg.sender, seller, amount, fee, deadline);
    }

    /// @notice Наблюдатель выпускает деньги продавцу после доказательства RIPE.
    /// @dev Продавец получает ЦЕЛИКОМ свою цену. Комиссия уходит площадке
    ///      и только здесь — то есть лишь когда сделка реально состоялась.
    ///      При возврате комиссии нет.
    function release(bytes32 dealId, bytes32 ripeKey) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Released;

        uint256 payout = d.amount;
        uint256 fee = d.fee;
        address seller = d.seller;

        if (!usdc.transfer(seller, payout)) revert TransferFailed();
        if (fee > 0) {
            if (!usdc.transfer(treasury, fee)) revert TransferFailed();
        }
        emit Released(dealId, seller, payout, fee, ripeKey);
    }

    /// @notice Наблюдатель возвращает деньги покупателю (факта нет / серая зона).
    /// @dev Возвращается ВСЁ внесённое, вместе с комиссией: за несостоявшуюся
    ///      сделку мы не берём ничего.
    function refund(bytes32 dealId) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount + d.fee;
        address buyer = d.buyer;
        bool ok = usdc.transfer(buyer, amount);
        if (!ok) revert TransferFailed();
        emit Refunded(dealId, buyer, amount);
    }

    /// @notice Если наблюдатель молчит и срок вышел — покупатель забирает сам.
    function refundAfterDeadline(bytes32 dealId) external {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        if (d.deadline == 0 || block.timestamp < d.deadline) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount + d.fee;
        address buyer = d.buyer;
        bool ok = usdc.transfer(buyer, amount);
        if (!ok) revert TransferFailed();
        emit Refunded(dealId, buyer, amount);
    }
}
