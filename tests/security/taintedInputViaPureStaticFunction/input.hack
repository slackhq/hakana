final class Utils {
    public static function shorten(string $str)[]: string {
        return substr($str, 0, 100);
    }
}

final class A {
    public function foo() : void {
        echo(Utils::shorten((string) HH\global_get('_GET')["user_id"]));
    }
}