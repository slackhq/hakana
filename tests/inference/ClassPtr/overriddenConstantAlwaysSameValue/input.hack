abstract class A {
    const bool ALWAYS_TRUE = true;
}

final class B extends A {
    const bool ALWAYS_TRUE = true;
}

function foo(class<A> $ptr): int {
    if ($ptr::ALWAYS_TRUE) {
        return 1;
    }

    return 0;
}
