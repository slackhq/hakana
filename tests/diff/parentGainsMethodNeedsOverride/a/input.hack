final class C extends P {
    public function foo(): void {}
}

<<__EntryPoint>>
function main(): void {
    (new C())->foo();
}
