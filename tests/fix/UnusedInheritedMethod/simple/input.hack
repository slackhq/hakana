abstract class Base {
    public function overridden(): void {}
}

final class Child extends Base {
    <<__Override>>
    public function overridden(): void {}
}

<<__EntryPoint>>
function main(): void {
    new Child();
}
