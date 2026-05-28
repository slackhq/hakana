<<__ConsistentConstruct>>
abstract class A {}

final class B extends A {}

final class C {}

final class D<<<__Newable>> reify TParam as A> {
    public function factory(): TParam {
        return new TParam();
    }

    public function bad(): TParam {
        return new C();
    }
}
