use HH\Lib\C;

abstract class A {}
final class B extends A {}

final class Other {}

function contains_class_ptr_literal(keyset<classname<A>> $classnames): bool {
    return C\contains($classnames, A::class) || C\contains($classnames, B::class);
}

function contains_class_ptr(keyset<classname<A>> $classnames, class<A> $ptr): bool {
    return C\contains($classnames, $ptr);
}

function mismatched_class_ptr_literal(keyset<classname<A>> $classnames): bool {
    return C\contains($classnames, Other::class);
}

function mismatched_class_ptr(keyset<classname<A>> $classnames, class<Other> $ptr): bool {
    return C\contains($classnames, $ptr);
}

function class_ptrs(vec<class<A>> $ptrs, class<B> $some_ptr, class<Other> $incompat_ptr): bool {
    // valid
    if (C\contains($ptrs, A::class) || C\contains($ptrs, $some_ptr)) {
        return true;
    }

    // incompatible class
    if (C\contains($ptrs, $incompat_ptr)) {
        return true;
    }

    // can't pass off a classname<A> as class<A>
    return C\contains($ptrs, nameof B);
}