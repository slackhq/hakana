final class A {}

function coerce<<<__Enforceable>> reify T as A>(A $a): T {
    $a as T;
    return $a;
}