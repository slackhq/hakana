interface A {}
interface I {}
class AChild implements I, A {}

function isAChild(A $value): ?AChild {
    if (!$value is I) {
        return null;
    }

    if (!$value is AChild) {
        return null;
    }

    return $value;
}