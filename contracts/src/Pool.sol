// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// A single constant-product pool with internal trader balances.
///
/// Every swap reads and writes both reserves, so a block of swaps on one pool
/// is a total-conflict workload by a different route than NFT minting: two hot
/// slots rather than one, and the output of each swap depends on the reserves
/// every earlier swap left behind.
///
/// Layout, relied on by the Rust generator: slot 0 `reserve0`, slot 1
/// `reserve1`, slot 2 `balance0`, slot 3 `balance1`.
contract Pool {
    uint256 public reserve0; // slot 0
    uint256 public reserve1; // slot 1
    mapping(address => uint256) public balance0; // slot 2
    mapping(address => uint256) public balance1; // slot 3

    error InsufficientBalance();

    constructor(uint256 r0, uint256 r1, address[] memory traders, uint256 amount) {
        reserve0 = r0;
        reserve1 = r1;
        for (uint256 i = 0; i < traders.length; i++) {
            balance0[traders[i]] = amount;
            balance1[traders[i]] = amount;
        }
    }

    /// Uniswap v2 pricing with the 0.3% fee.
    function swap(bool zeroForOne, uint256 amountIn) external returns (uint256 amountOut) {
        if (zeroForOne) {
            if (balance0[msg.sender] < amountIn) revert InsufficientBalance();
            amountOut = quote(amountIn, reserve0, reserve1);
            balance0[msg.sender] -= amountIn;
            balance1[msg.sender] += amountOut;
            reserve0 += amountIn;
            reserve1 -= amountOut;
        } else {
            if (balance1[msg.sender] < amountIn) revert InsufficientBalance();
            amountOut = quote(amountIn, reserve1, reserve0);
            balance1[msg.sender] -= amountIn;
            balance0[msg.sender] += amountOut;
            reserve1 += amountIn;
            reserve0 -= amountOut;
        }
    }

    function quote(uint256 amountIn, uint256 reserveIn, uint256 reserveOut) public pure returns (uint256) {
        uint256 inWithFee = amountIn * 997;
        return (inWithFee * reserveOut) / (reserveIn * 1000 + inWithFee);
    }
}
