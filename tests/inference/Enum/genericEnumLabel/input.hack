

final class Column<T> implements NamedColumn {
	public function __construct(public string $name)[] {}
}

enum class SomeTable: Column<arraykey> {
  Column<int> id = new Column("id");
  Column<string> name = new Column("name");
}

abstract class TWB<reify TKeys> {
	public function eq<T>(\HH\EnumClass\Label<TKeys, Column<T>> $column, T $val): void {}
}

final class SomeTableWB extends TWB<SomeTable> {
}

function full_print(SomeTableWB $wb): void {
    $wb->eq(#id, 20);
    $wb->eq(#id, 'hello');
}
