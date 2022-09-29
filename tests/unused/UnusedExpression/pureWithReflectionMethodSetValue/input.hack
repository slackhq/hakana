function foo(object $mock) : void {
    $m = new \ReflectionProperty($mock, "bar");
    $m->setValue(dict[get_class($mock) => "hello"]);
}