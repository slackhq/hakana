function bar(string $s): void { }

class A {
    public static dict<string, string> $a = dict[];
}

function foo(): void {
    $b = "hello";

    if (!isset(A::$a[$b])) {
        return;
    }

    bar(A::$a[$b]);
}