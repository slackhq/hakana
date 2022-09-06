enum Foo: int as int {
  FOO = 1;
  BAR = 2;
  BAZ = 3;
}

function takes_some_enum_name<T>(HH\enumname<T> $input): void {}

function takes_foo(Foo $f): void {}

function get_enum<T>(classname<BuiltinEnum<T>> $input): T {
    return /* HH_FIXME[4110] */ null;
}

function test(): void {
  takes_some_enum_name(Foo::class);
  $a = get_enum(Foo::class);
  takes_foo($a);
}
