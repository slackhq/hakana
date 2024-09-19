final class Utils {
    public static function shorten(string $str) : string {
        return $str;
    }
}

final class A {
    public function foo() : void {
        echo(Utils::shorten((string) HH\global_get('_GET')["user_id"]));
    }
}