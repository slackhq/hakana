

final class Column<T> implements NamedColumn {
	public function __construct(public string $name)[] {}
}

enum class SomeTableColumnType: Column<arraykey> {
  Column<int> id = new Column("id");
  Column<string> name = new Column("name");
}

abstract class TWB<reify TKeys> {
	public function eq<T>(\HH\EnumClass\Label<TKeys, Column<T>> $column, T $val): this {
    return $this;
  }
  public function filterWithOr((function(this): this) $or_lambda): this {
    return $this;
  }
}

function full_print(TWB<SomeTableColumnType> $wb): void {
    $wb->filterWithOr($or ==> $or->eq(#id, 5));
}
