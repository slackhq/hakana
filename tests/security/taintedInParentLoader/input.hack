abstract class A {
    abstract public static function loadPartial(string $sink) : void;

    public static function loadFull(string $sink) : void {
        static::loadPartial($sink);
    }
}

function getPdo() : AsyncMysqlConnection {
    return new AsyncMysqlConnection("connectionstring");
}

class AChild extends A {
    public static function loadPartial(string $sink) : void {
        getPdo()->query("select * from foo where bar = " . $sink);
    }
}

class AGrandChild extends AChild {}

class C {
    public function foo(string $user_id) : void {
        AGrandChild::loadFull($user_id);
    }
}

(new C)->foo((string) $_GET["user_id"]);