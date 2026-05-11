interface Unused {}

interface Used {}

final class Foo implements Used {}

<<__EntryPoint>>
function main(): void {
    new Foo();
}
