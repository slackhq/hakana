final class A {
    public ?string $name = null;
}

function foo(int $i, dict<int, A> $tokens) : void {
    if (isset($tokens[$i]->name) && $tokens[$i]->name === "hello") {}
}