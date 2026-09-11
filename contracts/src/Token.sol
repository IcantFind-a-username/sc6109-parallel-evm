// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// Minimal ERC-20 transfer logic for the conflict workloads.
///
/// The Rust workload generator seeds balances by writing storage directly, so
/// the layout below is load-bearing: slot 0 is the balance mapping, slot 1 the
/// total supply. `engine/tests/contracts.rs` verifies it by calling
/// `balanceOf` against seeded storage.
///
/// `transfer` reverts on insufficient balance. That makes a transaction's write
/// set depend on what it read — a success writes two balance slots, a revert
/// writes none — which is the only way the store's write retraction gets
/// exercised end to end (E8).
contract Token {
    mapping(address => uint256) public balanceOf; // slot 0
    uint256 public totalSupply; // slot 1

    event Transfer(address indexed from, address indexed to, uint256 value);

    error InsufficientBalance(uint256 available, uint256 required);

    constructor(address[] memory holders, uint256 amount) {
        for (uint256 i = 0; i < holders.length; i++) {
            balanceOf[holders[i]] = amount;
        }
        totalSupply = holders.length * amount;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        uint256 available = balanceOf[msg.sender];
        if (available < amount) revert InsufficientBalance(available, amount);
        unchecked {
            balanceOf[msg.sender] = available - amount;
        }
        balanceOf[to] += amount;
        emit Transfer(msg.sender, to, amount);
        return true;
    }
}
