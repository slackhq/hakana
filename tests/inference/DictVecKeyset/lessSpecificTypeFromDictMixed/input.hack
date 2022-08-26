function foo(shape('id' => string) $shape): void {}

function bar(dict<string, mixed> $dict): void {
    foo($dict);
}