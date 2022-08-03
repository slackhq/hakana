interface Foo {
    public function doFoo(): int;
}

class Bar implements Foo {
    public function doFoo(): int {
      print "Error\n";
      exit(1);
    }
}

class Baz implements Foo {
    public function doFoo(): int {
        throw new \Exception("bad");
    }
}