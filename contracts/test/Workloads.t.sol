// SPDX-License-Identifier: MIT
pragma solidity 0.8.28;

import {Token} from "../src/Token.sol";
import {Collectible} from "../src/Collectible.sol";
import {Pool} from "../src/Pool.sol";

/// Stands in for an externally owned account, so tests can act as distinct
/// senders without cheatcodes. forge-std is deliberately not a dependency.
contract Actor {
    function transfer(Token t, address to, uint256 amount) external returns (bool) {
        return t.transfer(to, amount);
    }

    function mint(Collectible c) external returns (uint256) {
        return c.mint();
    }

    function swap(Pool p, bool zeroForOne, uint256 amountIn) external returns (uint256) {
        return p.swap(zeroForOne, amountIn);
    }
}

contract TokenTest {
    Token token;
    Actor alice;
    Actor bob;

    function setUp() public {
        alice = new Actor();
        bob = new Actor();
        address[] memory holders = new address[](1);
        holders[0] = address(alice);
        token = new Token(holders, 100);
    }

    function test_transfer_moves_balance() public {
        alice.transfer(token, address(bob), 40);
        require(token.balanceOf(address(alice)) == 60, "sender");
        require(token.balanceOf(address(bob)) == 40, "recipient");
        require(token.totalSupply() == 100, "supply unchanged");
    }

    function test_transfer_reverts_on_insufficient_balance() public {
        try alice.transfer(token, address(bob), 101) {
            revert("expected a revert");
        } catch {}
        require(token.balanceOf(address(alice)) == 100, "revert must leave balances untouched");
    }

    function test_self_transfer_conserves_balance() public {
        alice.transfer(token, address(alice), 30);
        require(token.balanceOf(address(alice)) == 100, "self transfer");
    }
}

contract CollectibleTest {
    function test_mint_assigns_sequential_ids() public {
        Collectible c = new Collectible();
        Actor a = new Actor();
        Actor b = new Actor();
        require(a.mint(c) == 0, "first id");
        require(b.mint(c) == 1, "second id");
        require(a.mint(c) == 2, "third id");
        require(c.totalSupply() == 3, "supply");
        require(c.ownerOf(1) == address(b), "owner");
        require(c.balanceOf(address(a)) == 2, "balance");
    }
}

contract PoolTest {
    Pool pool;
    Actor trader;

    function setUp() public {
        trader = new Actor();
        address[] memory traders = new address[](1);
        traders[0] = address(trader);
        pool = new Pool(1_000_000, 1_000_000, traders, 10_000);
    }

    function test_swap_follows_constant_product_with_fee() public {
        uint256 out = trader.swap(pool, true, 1_000);
        uint256 amountIn = 1_000;
        uint256 reserve = 1_000_000;
        // Integer division, as on-chain: literals alone would be exact rationals.
        require(out == (amountIn * 997 * reserve) / (reserve * 1_000 + amountIn * 997), "quote");
        require(pool.reserve0() == 1_001_000, "reserve0");
        require(pool.reserve1() == 1_000_000 - out, "reserve1");
        require(pool.balance0(address(trader)) == 9_000, "balance0");
        require(pool.balance1(address(trader)) == 10_000 + out, "balance1");
    }

    function test_product_never_decreases() public {
        uint256 before = pool.reserve0() * pool.reserve1();
        trader.swap(pool, true, 5_000);
        trader.swap(pool, false, 2_000);
        require(pool.reserve0() * pool.reserve1() >= before, "k must not shrink");
    }

    function test_swap_reverts_on_insufficient_balance() public {
        try trader.swap(pool, true, 10_001) {
            revert("expected a revert");
        } catch {}
        require(pool.reserve0() == 1_000_000, "reserves untouched");
    }
}
