final class A {}

function foo(int $a): void {
    if (rand(0, 1) || $a is A) {}
}