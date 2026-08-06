// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "./IERC20.sol";
import {IAPassComplianceValidator} from "./IAPassComplianceValidator.sol";

/// @title EscrowNad proof-escrow lock (Monad) — **USDC first-class**
/// @notice Buyer deposits **USDC (ERC-20)** after `approve`; observer releases/refunds USDC.
/// @dev Settlement asset = USDC. Native MON is gas only (not deal funds).
///      Constructor takes `usdc` token + `observer` EOA (Rust service, v1 single key).
///      Dev: deploy MockUSDC then EscrowLock(mockUsdc, observer).
///
///      Identity: both sides of a deal must hold a Cleanverse Verified Identity
///      (CVI). The check happens in `fund`, against the CCP validator, and it
///      covers the seller too — not only the wallet paying. See `setValidator`
///      for why the gate is switched on after deployment rather than in the
///      constructor.
contract EscrowLock {
    address public owner;
    address public observer;
    /// @notice Cleanverse CCP validator. Zero address = identity gate is off.
    /// @dev Not immutable on purpose: a pool is registered with the validator
    ///      by its own address, which does not exist until the contract is
    ///      deployed. So the order is deploy → register the pool through the
    ///      Cleanverse API → point the contract at the validator here.
    IAPassComplianceValidator public validator;
    /// @notice Where the platform fee goes.
    address public treasury;
    /// @notice Insurance fund: builds up a reserve for disputed cases.
    /// @dev A separate address on purpose — the fund's balance is visible on-chain to anyone.
    address public insurance;
    IERC20 public immutable usdc;

    /// @notice Platform fee, in hundredths of a percent (basis points). 100 = 1%.
    uint16 public feeBps = 100;
    /// @notice Contribution to the insurance fund, in hundredths of a percent. 100 = 1%.
    uint16 public insuranceBps = 100;

    /// @dev ONLY the buyer pays — the party parting with the money. The buyer
    ///      deposits the price plus both charges on top. The asset owner
    ///      (IP block, goods) receives exactly their price and pays nothing.
    ///      The combined charges are capped at 5% and hardcoded into the
    ///      contract: the owner cannot crank it up arbitrarily — this is
    ///      protection of the users from us.
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
        uint256 amount; // deal price, USDC base units (6 decimals)
        uint256 fee; // platform fee, deposited by the buyer on top of the price
        uint256 insuranceFee; // insurance fund contribution, also on top of the price
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
    event ValidatorChanged(address validator);

    error NotOwner();
    error NotObserver();
    error BadState();
    error BadValue();
    error ZeroAddress();
    error TransferFailed();
    error FeeTooHigh();
    /// @notice Raised when a party to the deal holds no qualifying CVI.
    /// @param who The wallet that failed the check — buyer or seller.
    error NotCompliant(address who);

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyObserver() {
        if (msg.sender != observer) revert NotObserver();
        _;
    }

    /// @param usdc_ Settlement token (USDC, 6 decimals).
    /// @param observer_ EOA of the observer service that releases and refunds deals.
    /// @param treasury_ Recipient of the platform fee.
    /// @param insurance_ Address of the insurance fund.
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

    /// @notice Replaces the observer key. Owner only.
    /// @param observer_ New observer address.
    function setObserver(address observer_) external onlyOwner {
        if (observer_ == address(0)) revert ZeroAddress();
        observer = observer_;
    }

    /// @notice Points the contract at the Cleanverse CCP validator, switching the
    ///         identity gate on.
    /// @dev Deliberately a separate step rather than a constructor argument. A
    ///      compliance pool is registered *by its own address*, and that address
    ///      only exists once the contract is on chain — so the gate cannot be
    ///      wired up at construction time without locking the contract out of
    ///      its own registration. Passing the zero address turns the gate off,
    ///      which is what a fresh deployment starts with.
    /// @param validator_ Validator address, or zero to disable the gate.
    function setValidator(address validator_) external onlyOwner {
        validator = IAPassComplianceValidator(validator_);
        emit ValidatorChanged(validator_);
    }

    /// @notice May this wallet take part in a deal?
    /// @dev Answers true when the gate is off, so the frontend can call this
    ///      unconditionally and render the same way in both modes.
    /// @param who Wallet to check.
    /// @return True when the wallet passes, or when no validator is set.
    function isCompliant(address who) public view returns (bool) {
        if (address(validator) == address(0)) return true;
        return validator.complianceVerify(address(this), who);
    }

    /// @notice Replaces every compliance rule attached to this contract's pool.
    /// @param rule The rule that becomes the only one.
    function setRuleV2FromContract(IAPassComplianceValidator.RuleV2 calldata rule)
        external
        onlyOwner
    {
        validator.setRuleV2FromContract(rule);
    }

    /// @notice Adds one more rule; a wallet matching any rule passes.
    /// @param rule The rule to append.
    function addRuleV2FromContract(IAPassComplianceValidator.RuleV2 calldata rule)
        external
        onlyOwner
    {
        validator.addRuleV2FromContract(rule);
    }

    /// @notice Drops a rule by its index.
    /// @param index Position of the rule to remove.
    function removeRuleV2FromContract(uint256 index) external onlyOwner {
        validator.removeRuleV2FromContract(index);
    }

    /// @notice Lists the compliance rules currently in force for this contract.
    function getRulesV2() external view returns (IAPassComplianceValidator.RuleV2[] memory) {
        return validator.getRulesV2(address(this));
    }

    /// @notice Changes the charges and their recipients. The charges must add up to no more than 5%.
    /// @dev Does NOT affect already funded deals: their amounts were computed
    ///      and written down at the moment of `fund`. They cannot be changed
    ///      after the fact.
    /// @param feeBps_ New platform fee, in hundredths of a percent.
    /// @param insuranceBps_ New insurance fund contribution, in hundredths of a percent.
    /// @param treasury_ New recipient of the platform fee.
    /// @param insurance_ New address of the insurance fund.
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

    /// @notice How much the buyer transfers in total: price + platform fee + insurance.
    /// @dev The frontend calls this before `approve`, to show the full amount.
    /// @param amount Deal price, USDC base units (6 decimals).
    /// @return total Full amount pulled from the buyer: amount + fee + insuranceFee.
    /// @return fee Platform fee derived from the current `feeBps`.
    /// @return insuranceFee Insurance fund contribution derived from the current `insuranceBps`.
    function quote(uint256 amount)
        public
        view
        returns (uint256 total, uint256 fee, uint256 insuranceFee)
    {
        fee = (amount * feeBps) / 10_000;
        insuranceFee = (amount * insuranceBps) / 10_000;
        total = amount + fee + insuranceFee;
    }

    /// @notice The buyer deposits the price + their own charges. Before the call,
    ///         `usdc.approve(this, total)` is required, where total = quote(amount).total.
    /// @param dealId Deal identifier (hash of the deal).
    /// @param seller Recipient of the price once the deal goes through.
    /// @param amount Deal price, USDC base units (6 decimals); the charges are added on top.
    /// @param deadline Unix seconds; 0 = no on-chain timeout hatch.
    /// @param conditionHash Hash of the condition being settled; nothing but the fingerprint goes on-chain.
    /// @dev Both parties are checked against the CCP validator here, and this is
    ///      the only place money enters the contract — so an unverified wallet
    ///      cannot end up on either side of a deal. The seller is checked as
    ///      well as the buyer: knowing who receives the money matters at least
    ///      as much as knowing who sends it.
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

        if (address(validator) != address(0)) {
            if (!validator.complianceVerify(address(this), msg.sender)) {
                revert NotCompliant(msg.sender);
            }
            if (!validator.complianceVerify(address(this), seller)) {
                revert NotCompliant(seller);
            }
        }

        (uint256 total, uint256 fee, uint256 insuranceFee) = quote(amount);

        d.seller = seller;
        d.buyer = msg.sender;
        d.amount = amount;
        // the charges are pinned down right here — they cannot be changed after the fact
        d.fee = fee;
        d.insuranceFee = insuranceFee;
        d.deadline = deadline;
        d.state = State.Funded;
        d.conditionHash = conditionHash;

        bool ok = usdc.transferFrom(msg.sender, address(this), total);
        if (!ok) revert TransferFailed();

        emit Funded(dealId, msg.sender, seller, amount, fee, insuranceFee, deadline);
    }

    /// @notice The observer releases the money to the seller after the RIPE proof.
    /// @dev The seller receives their price IN FULL. The fee goes to the platform
    ///      here and only here — that is, only once the deal has actually gone
    ///      through. On a refund there is no fee.
    /// @param dealId Deal identifier passed to `fund`.
    /// @param ripeKey Key of the proof that triggered the release; emitted in the event.
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

    /// @notice The observer returns the money to the buyer (no fact / grey zone).
    /// @dev EVERYTHING deposited is returned, the charges included: for a deal
    ///      that did not go through we take nothing.
    /// @param dealId Deal identifier passed to `fund`.
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

    /// @notice If the observer stays silent and the deadline has passed, the buyer takes the money back themselves.
    /// @dev This is the buyer's escape hatch: it needs no observer, no owner and
    ///      no cooperation from us. Whatever happens to the platform, money in a
    ///      lock past its deadline can always be recovered by the person who put
    ///      it there.
    ///
    ///      On `block.timestamp`: a validator can nudge it by seconds, and the
    ///      linter rightly flags comparisons against it. Here it does not
    ///      matter — deadlines on this market are measured in days, because a
    ///      registry transfer runs through a registrar and takes that long. A
    ///      few seconds either way changes nothing, and there is no incentive to
    ///      manipulate: the only thing an earlier timestamp buys is letting the
    ///      buyer reclaim their own deposit slightly sooner.
    /// @param dealId Deal identifier passed to `fund`.
    function refundAfterDeadline(bytes32 dealId) external {
        Deal storage d = deals[dealId];
        if (d.state != State.Funded) revert BadState();
        // forge-lint: disable-next-line(block-timestamp)
        if (d.deadline == 0 || block.timestamp < d.deadline) revert BadState();
        d.state = State.Refunded;
        uint256 amount = d.amount + d.fee + d.insuranceFee;
        address buyer = d.buyer;
        bool ok = usdc.transfer(buyer, amount);
        if (!ok) revert TransferFailed();
        emit Refunded(dealId, buyer, amount);
    }
}
