final class Foo {
    public int $unused = 0;
    public int $used = 1;
}

<<__EntryPoint>>
function main(): void {
    $f = new Foo();
    echo $f->used;
}
