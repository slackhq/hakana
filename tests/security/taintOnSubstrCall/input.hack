final class U {
    public static function shorten(string $s)[]: string {
        return substr($s, 0, 15);
    }
}

final class V {}

final class O1 {
    public string $s;

    public function __construct() {
        $this->s = (string) $_GET["FOO"];
    }
}

final class V1 extends V {
    public function foo(O1 $o) : void {
        echo U::shorten($o->s);
    }
}

(new V1())->foo(new O1());