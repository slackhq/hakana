abstract class Base {
    public function foo(): void {}
}

final class Child extends Base {
    <<__Override>>
    public function foo(): void {}
}
