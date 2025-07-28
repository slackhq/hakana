final class Foo {
    public function __construct(private string $s) {}

    public function getImpure(): string {
        echo rand(0, 1);
        return $this->s;
    }
}

function bar(Foo $f) {
    $val = $f->getImpure();
    echo $val;
}