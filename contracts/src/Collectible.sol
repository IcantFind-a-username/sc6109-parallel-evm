// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

/// NFT minting reduced to its conflict structure.
///
/// Every `mint` reads and writes `totalSupply`, so every transaction in a mint
/// block depends on the one before it: the critical path is the block, and the
/// parallelism ceiling is exactly one. Whatever a parallel scheduler measures
/// below 1.0x here is pure coordination overhead.
///
/// Layout, relied on by the Rust generator: slot 0 `totalSupply`, slot 1
/// `ownerOf`, slot 2 `balanceOf`.
contract Collectible {
    uint256 public totalSupply; // slot 0 — the hot slot
    mapping(uint256 => address) public ownerOf; // slot 1
    mapping(address => uint256) public balanceOf; // slot 2

    function mint() external returns (uint256 id) {
        id = totalSupply;
        totalSupply = id + 1;
        ownerOf[id] = msg.sender;
        balanceOf[msg.sender] += 1;
    }
}
