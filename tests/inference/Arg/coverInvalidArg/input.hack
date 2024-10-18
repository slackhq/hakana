function foo(int $a, int $b, int $c): void {}

function main(): void {
    foo('a', /* HH_FIXME[4110] */ 'b', 'c');
}
