final class A {
    public function foo(vec<string> $vecs): void {
        echo "a";
        /* HAKANA_FIXME[RedundantNonnullTypeComparison] */
        if ($vecs is nonnull) {}
        if ($vecs is nonnull) {}
    }
}

<<__EntryPoint>>
function main(): void {
    (new A())->foo();
}