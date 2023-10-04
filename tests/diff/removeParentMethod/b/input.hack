class B extends A {
    public function bar(): void {
        $this->foo();
    }
}

<<__EntryPoint>>
function main(): void {
    (new B())->bar();
}