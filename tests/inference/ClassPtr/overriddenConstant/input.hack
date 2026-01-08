abstract class A {
    const bool MAYBE_TRUE = false;
}

final class B extends A {
    const bool MAYBE_TRUE = true;
}

function foo(class<A> $ptr): int {
    if ($ptr::MAYBE_TRUE) {
        return 1;
    }

    return 0;
}
