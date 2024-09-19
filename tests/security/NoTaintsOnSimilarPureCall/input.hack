final class U {
    public static function shorten(string $s)[] : string {
        return substr($s, 0, 15);
    }

    public static function escape(string $s)[] : string {
        return htmlentities($s, \ENT_QUOTES);
    }
}

final class O1 {
    public string $s;

    public function __construct(string $s) {
        $this->s = $s;
    }
}

final class O2 {
    public string $t;

    public function __construct() {
        $this->t = (string) HH\global_get('_GET')["FOO"];
    }
}

final class V1 {
    public function foo() : void {
        $o = new O1((string) HH\global_get('_GET')["FOO"]);
        echo U::escape(U::shorten($o->s));
    }
}

final class V2 {
    public function foo(O2 $o) : void {
        echo U::shorten(U::escape($o->t));
    }
}