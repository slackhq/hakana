final class Utils {
    public static function shorten(string $str)[] : string {
        return $str;
    }
}

final class A {
    public function foo() : void {
        echo(htmlentities(Utils::shorten((string) $_GET["user_id"]), \ENT_QUOTES));
    }

    public function bar() : void {
        echo(Utils::shorten("hello"));
    }
}