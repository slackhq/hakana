abstract class P {
    public function foo(): void {}
}

final class C extends P {
    <<__Override>>
    public function foo(): void {}
}
