function bar(string $s): void { }

final class A {
    public static dict<string, string> $a = dict[];
}

function foo(): void {
    $b = "hello";

    if (!isset(A::$a[$b])) {
        $g = "bar";
    } else {
        bar(A::$a[$b]);
        $g = "foo";
    }

    bar($g);
}