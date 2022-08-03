class A {}
class B extends A {}
function foo(A $left, A $right) : void {
    if (($left is B && rand(0, 1))
        || ($right is B && rand(0, 1))
    ) {
        if ($left is B
            && rand(0, 1)
            && $right is B
            && rand(0, 1)
        ) {}
    }
}