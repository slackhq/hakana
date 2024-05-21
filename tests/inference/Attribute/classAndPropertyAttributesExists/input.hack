namespace Foo;

final class Table implements HH\ClassAttribute {
    public function __construct(public string $name) {}
}

final class Column implements HH\PropertyAttribute {
    public function __construct(public string $name) {}
}

<<Table("videos")>>
final class Video {
    <<Column("id")>>
    public string $id = "";

    <<Column("title")>>
    public string $name = "";
}

<<Table("users")>>
final class User {
    public function __construct(
        <<Column("id")>>
        public string $id,

        <<Column("name")>>
        public string $name = "",
    ) {}
}