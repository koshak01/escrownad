// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title Cleanverse CVI compliance validator
/// @notice On-chain identity gate of the Cleanverse Compliance Protocol (CCP).
///         A business contract registers itself as a compliance pool, attaches
///         one or more RuleV2 policies to it, and then asks the validator
///         whether a given wallet satisfies any of them.
/// @dev Only the subset EscrowNad actually uses is declared here. The pool is
///      registered off-chain through `POST /api/cooperate/validator/register`,
///      signed by the contract owner — a contract cannot register itself.
interface IAPassComplianceValidator {
    /// @notice A single compliance policy.
    /// @dev Fields inside one rule are combined with AND; several rules attached
    ///      to the same pool are combined with OR. A zero value means the field
    ///      places no restriction at all.
    struct RuleV2 {
        bytes2 allowedGroup; // required CVI group, 0x0000 = any
        bytes2 allowedSubGroup; // required CVI sub-group, 0x0000 = any
        uint8 minTier; // minimum CVI tier, 0 = any, range 0-99
        uint8 minSubTier; // minimum CVI sub-tier, 0 = any, range 0-99
        uint256 poolCountryBitmap; // allowed countries as a bitmap, 0 = any
    }

    /// @notice Does this wallet satisfy the rules attached to this pool?
    /// @dev Permissionless view call — anyone may ask, and it costs nothing.
    /// @param poolAddress Registered pool, i.e. the business contract itself.
    /// @param userAddress Wallet being checked.
    /// @return True when the wallet holds a valid CVI matching at least one rule.
    function complianceVerify(address poolAddress, address userAddress)
        external
        view
        returns (bool);

    /// @notice Is this address registered as a compliance pool?
    function isRegistered(address poolAddress) external view returns (bool);

    /// @notice Replaces every rule attached to the calling contract's pool.
    function setRuleV2FromContract(RuleV2 calldata rule) external;

    /// @notice Appends one more rule to the calling contract's pool (OR semantics).
    function addRuleV2FromContract(RuleV2 calldata rule) external;

    /// @notice Removes a rule from the calling contract's pool by index.
    function removeRuleV2FromContract(uint256 index) external;

    /// @notice Lists the rules currently attached to a pool.
    function getRulesV2(address poolAddress) external view returns (RuleV2[] memory);
}
