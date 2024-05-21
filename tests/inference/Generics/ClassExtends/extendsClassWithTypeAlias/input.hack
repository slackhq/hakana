interface Foo<T> {}

type MyString = string;

final class MyFoo implements Foo<MyString> {}

function takesFoo(Foo<MyString> $foo): void {}

function takesMyFoo(MyFoo $foo): void {
    takesFoo($foo);
}