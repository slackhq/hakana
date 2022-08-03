class A {}
interface I {}
class AChild extends A implements I {}

function isAChild(A $value): ?AChild {
    if (!$value is I) {
        return null;
    }

    if (!$value is AChild) {
        return null;
    }

    return $value;
}