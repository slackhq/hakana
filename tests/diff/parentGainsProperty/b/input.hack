final class C extends P {
    public function m(): int {
        return $this->prop;
    }
}

<<__EntryPoint>>
function main(): void {
    (new C())->m();
}
