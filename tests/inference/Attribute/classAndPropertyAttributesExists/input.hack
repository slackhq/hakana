namespace Foo;

class Table implements HH\ClassAttribute {
    public function __construct(public string $name) {}
}

class Column implements HH\PropertyAttribute {
    public function __construct(public string $name) {}
}

<<Table("videos")>>
class Video {
    <<Column("id")>>
    public string $id = "";

    <<Column("title")>>
    public string $name = "";
}

<<Table("users")>>
class User {
    public function __construct(
        <<Column("id")>>
        public string $id,

        <<Column("name")>>
        public string $name = "",
    ) {}
}