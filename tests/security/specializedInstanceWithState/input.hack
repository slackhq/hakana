final class A {
    private string $taint = "";
    private static string $stub = "";

    public function __construct(string $taint) {
        $this->taint = $taint;
    }

    public static function setStub(string $s): void {
        self::$stub = $s;
    }

    public function getTaint() : string {
        return self::$stub . $this->taint;
    }
}

$a = new A("a");
A::setStub(HH\global_get('_GET')["foo"]);
echo $a->getTaint();