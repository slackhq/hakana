final class Foo {
    /** cool doc */
    private function bar(): void {}

    private function baz(): void {
        echo "got here";
    }
}

<<__EntryPoint>>
function main(): void {
    new Foo();
}