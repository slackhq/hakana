<<__EntryPoint>>
function main(): void {
    A::takes_bar(shape('a' => Foo::A));
}

final class A {
    public static function takes_bar(bar_t $b) {
        if ($b['a'] == Foo::A) {}
    }
}
