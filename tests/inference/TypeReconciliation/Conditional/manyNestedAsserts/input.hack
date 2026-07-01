abstract class A {}
final class B extends A {}
function foo(A $left, A $right) : void {
    if (($left is B && rand(0, 1) !== 0)
        || ($right is B && rand(0, 1) !== 0)
    ) {
        if ($left is B
            && rand(0, 1) !== 0
            && $right is B
            && rand(0, 1) !== 0
        ) {}
    }
}