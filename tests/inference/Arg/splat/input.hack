abstract class Base {}

final class Child1 extends Base {}

final class Child2 extends Base {}

final class Other {}

function splat_fn<Targ as (Base...)>(int $x, ...Targ $args): void {}

function mixed_splat_fn<Targ as (mixed...)>(int $y, ...Targ $args): void {}

function splat_multi_fn<Targ as (Other, Base...)>(int $z, ...Targ $args): void {}

function foo(): void {
    $child1 = new Child1();
    $child2 = new Child2();

    // fine, splat argument absent or matches
    splat_fn(1);
    splat_fn(2, $child1);
    splat_fn(3, $child1, $child2);

    // mismatch
    splat_fn(4, new Other());

    // all fine due to mixed constraint
    mixed_splat_fn(5, 6, new Other());
    mixed_splat_fn(7, new Child1());

    // fine, both the variadic and fixed tuple args match
    splat_multi_fn(8);
    splat_multi_fn(9, new Other());
    splat_multi_fn(10, new Other(), new Child1());

    // bad
    splat_multi_fn(11, new Child1());
}
