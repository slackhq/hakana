final class A {}

function coerce<<<__Enforceable>> reify T as A>(A $a): T {
    $b = $a ?as T;

    if ($b is null) {
        throw new \Exception('bad');
    }

    return $b;
}