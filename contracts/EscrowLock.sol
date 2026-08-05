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
    /// @notice Страховой фонд: копит резерв на спорные случаи.
    /// @dev Отдельный адрес специально — баланс фонда виден в цепи любому.
    address public insurance;
    IERC20 public immutable usdc;

    /// @notice Комиссия площадки, в сотых долях процента. 100 = 1%.
    uint16 public feeBps = 100;
    /// @notice Отчисление в страховой фонд, в сотых долях процента. 100 = 1%.
    uint16 public insuranceBps = 100;

    /// @dev Платит ТОЛЬКО покупатель — тот, кто отдаёт деньги. Он вносит
    ///      цену плюс оба сбора сверху. Владелец актива (IP-блока, товара)
    ///      получает ровно свою цену и не платит ничего.
    ///      Сумма сборов ограничена 5% и зашита в контракт: владелец не
    ///      может задрать её произвольно, это защита пользователей от нас.
    uint16 public constant MAX_TOTAL_BPS = 500;

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
        uint256 insuranceFee; // отчисление в страховой фонд, тоже сверх цены
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
        uint256 insuranceFee,
        uint64 deadline
    );
    event Released(
        bytes32 indexed dealId,
        address indexed seller,
        uint256 paidToSeller,
        uint256 feeTaken,
        uint256 insuranceTaken,
        bytes32 ripeKey
    );
    event Refunded(bytes32 indexed dealId, address indexed buyer, uint256 amount);
    event FeeChanged(uint16 feeBps, uint16 insuranceBps, address treasury, address insurance);

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

    constructor(
        address usdc_,
        address observer_,
        address treasury_,
        address insurance_
    ) {
        if (
            usdc_ == address(0) || observer_ == address(0)
                || treasury_ == address(0) || insurance_ == address(0)
        ) {
            revert ZeroAddress();
        }
        owner = msg.sender;
        usdc = IERC20(usdc_);
        observer = observer_;
        treasury = treasury_;
        insurance = insurance_;
    }

    function setObserver(address observer_) external onlyOwner {
        if (observer_ == address(0)) revert ZeroAddress();
        observer = observer_;
    }

    /// @notice Меняет сборы и получателей. Сумма сборов не выше 5%.
    /// @dev На уже профинансированные сделки НЕ влияет: суммы посчитаны
    ///      и записаны в момент fund. Задним числом их изменить нельзя.
    function setFee(
        uint16 feeBps_,
        uint16 insuranceBps_,
        address treasury_,
        address insurance_
    ) external onlyOwner {
        if (uint256(feeBps_) + uint256(insuranceBps_) > MAX_TOTAL_BPS) revert FeeTooHigh();
        if (treasury_ == address(0) || insurance_ == address(0)) revert ZeroAddress();
        feeBps = feeBps_;
        insuranceBps = insuranceBps_;
        treasury = treasury_;
        insurance = insurance_;
        emit FeeChanged(feeBps_, insuranceBps_, treasury_, insurance_);
    }

    /// @notice Сколько покупатель переведёт всего: цена + комиссия + страховка.
    /// @dev Фронт зовёт это перед approve, чтобы показать полную сумму.
    function quote(uint256 amount)
        public
        view
        returns (uint256 total, uint256 fee, uint256 insuranceFee)
    {
        fee = (amount * feeBps) / 10_000;
        insuranceFee = (amount * insuranceBps) / 10_000;
        total = amount + fee + insuranceFee;
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

        (uint256 total, uint256 fee, uint256 insuranceFee) = quote(amount);

        d.seller = seller;
        d.buyer = msg.sender;
        d.amount = amount;
        // сборы фиксируем здесь же — задним числом их изменить нельзя
        d.fee = fee;
        d.insuranceFee = insuranceFee;
        d.deadline = deadline;
        d.state = State.Funded;
        d.conditionHash = conditionHash;

        bool ok = usdc.transferFrom(msg.sender, address(this), total);
        if (!ok) revert TransferFailed();

        emit Funded(dealId, msg.sender, seller, amount, fee, insuranceFee, deadline);
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
        uint256 insuranceFee = d.insuranceFee;
        address seller = d.seller;

        if (!usdc.transfer(seller, payout)) revert TransferFailed();
        if (fee > 0) {
            if (!usdc.transfer(treasury, fee)) revert TransferFailed();
        }
        if (insuranceFee > 0) {
            if (!usdc.transfer(insurance, insuranceFee)) revert TransferFailed();
        }
        emit Released(dealId, seller, payout, fee, insuranceFee, ripeKey);
    }

    /// @notice Наблюдатель возвращает деньги покупателю (факта нет / серая зона).
    /// @dev Возвращается ВСЁ внесённое, вместе с комиссией: за несостоявшуюся
    ///      сделку мы не берём ничего.
    function refund(bytes32 dealId) external onlyObserver {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount + d.fee + d.insuranceFee;
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
        uint256 amount = d.amount + d.fee + d.insuranceFee;
        address buyer = d.buyer;
        bool ok = usdc.transfer(buyer, amount);
        if (!ok) revert TransferFailed();
        emit Refunded(dealId, buyer, amount);
    }
}
