final class A {}

final class B<T> {}

final class C<T> {
    public function __construct(class<B<T>> $cls) {}
}

final class D {
    public function generic_method<T>(class<B<T>> $cls): C<T> {
        return new C<T>($cls);
    }
}


