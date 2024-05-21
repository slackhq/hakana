interface A {}
final class B extends Exception {}

function foo(Throwable $e): void {
    if ($e is A || $e is B) {
        return;
    }

    return;
}

final class C extends Exception {}
interface D {}

function bar(Throwable $e): void {
    if ($e is C || $e is D) {
        return;
    }

    return;
}