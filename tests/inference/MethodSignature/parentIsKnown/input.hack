abstract class A {
    public function returnSelf() : A {
        return $this;
    }
}

final class B extends A {
    <<__Override>>
    public function returnSelf() : A {
        return parent::returnSelf();
    }

}