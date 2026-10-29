type Data = shape('s' => string);
function id<T>(T $v): T { return $v; }
function source<T>(): vec<T> { throw new Exception(); }
function simple(Data $v): Data { return id<Data>($v); }
function vector(vec<Data> $v): vec<Data> { return id<vec<Data>>($v); }
function nested(): vec<Data> { return id(source<Data>()); }
function typed(): vec<Data> { return source<Data>(); }
final class Holder<T> { public function __construct(public T $value) {} }
function held(Data $v): Holder<Data> { return new Holder<Data>($v); }

abstract class Store<T> {
    public function read(vec<T> $rows): vec<T> { return $rows; }
}
final class DataStore extends Store<Data> {}
function read_rows(vec<Data> $rows): vec<Data> {
    return (new DataStore())->read(vec($rows));
}
