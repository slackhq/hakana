class A11 {
    public function call() : A11 {
        $result = self::method();
        return $result;
    }

    public function method() : A11 {
        return $this;
    }
}
$x = new A11();
var_export($x->call());