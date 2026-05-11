final class Foo {
    public function unused(): void {}

    public function used(): void {
        echo "used";
    }
}

<<__EntryPoint>>
function main(): void {
    $f = new Foo();
    $f->used();
}
