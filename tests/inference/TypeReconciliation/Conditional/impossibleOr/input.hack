final class A {}

function foo(int $a): void {
    if (rand(0, 1) !== 0 || $a is A) {}
}