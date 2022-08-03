class A {
    public function returnSelf() : A {
        return $this;
    }
}

class B extends A {
    public function returnSelf() : A {
        return parent::returnSelf();
    }

}