final class Foo {
    private int $unused = 0;
    private int $used = 1;

    public function bar(): int {
        return $this->used;
    }
}

<<__EntryPoint>>
function main(): void {
    $f = new Foo();
    echo $f->bar();
}
