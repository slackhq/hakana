abstract class A {
    abstract public static function loadPartial(string $sink) : void;

    public static function loadFull(string $sink) : void {
        static::loadPartial($sink);
    }
}

function getPdo() : AsyncMysqlConnection {
    return new AsyncMysqlConnection("connectionstring");
}

final class AChild extends A {
    <<__Override>>
    public static function loadPartial(string $sink) : void {
        getPdo()->query("select * from foo where bar = " . $sink);
    }
}

final class AGrandChild extends AChild {}

final class C {
    public function foo(string $user_id) : void {
        AGrandChild::loadFull($user_id);
    }
}

(new C())->foo((string) HH\global_get('_GET')["user_id"]);