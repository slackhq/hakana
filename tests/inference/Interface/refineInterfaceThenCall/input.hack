interface I {
    public function foo(): arraykey;
}

interface IChild extends I {
    public function foo(): string;
}

function bar(I $i): string {
    if ($i is IChild) {
        return $i->foo();
    } else {
        return '';
    }
}