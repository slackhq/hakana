class A {}
class B {}

function foo<T>(typename<T> $t): void {
    if ($t === A::class) {
    } else if ($t === B::class) {}
}