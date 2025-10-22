abstract class A {}

final class B extends A {}

final class C {

}

function takes_classname(classname<A> $cls): void {
}

function takes_class_ptr(class<A> $cls): void {
}

function foo(): void {
    // typechecker error if class_class_type=true
    takes_classname(A::class);
    takes_classname(B::class);

    // valid
    takes_classname(nameof B);
    takes_class_ptr(A::class);
    takes_class_ptr(B::class);
    takes_class_ptr(C::class);

    // wrong class
    takes_classname(C::class);
    takes_classname(nameof C);

    // can't pass off a classname<T> as a class<T>
    takes_class_ptr(nameof B);
}