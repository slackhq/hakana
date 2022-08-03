class A {}
class B extends A {}

function foo(A $a): void {
    if (get_class($a) === A::class) {}
}