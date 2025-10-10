abstract class O {}
final class Foo extends O {}

final class FooCollection extends Iterator<int, Foo> {
    private function iterate() : void {
        foreach ($this as $foo) {}
    }
    <<__Override>>
    public function current() { return new Foo(); }
    <<__Override>>
    public function key(): int { return 0; }
    <<__Override>>
    public function next(): void {}
    <<__Override>>
    public function rewind(): void {}
    <<__Override>>
    public function valid(): bool { return false; }
}