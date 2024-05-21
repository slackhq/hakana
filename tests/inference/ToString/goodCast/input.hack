final class A {
    public function __toString(): string
    {
        return "hello";
    }
}

function fooFoo(string $b): void {}

fooFoo((string)new A());
