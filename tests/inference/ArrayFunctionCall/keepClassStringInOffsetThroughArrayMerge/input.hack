
final class A {
    private dict<classname<A>, string> $a;

    public function __construct() {
        $this->a = dict[];
    }

    public function handle(): void {
        $b = dict[nameof A => "d"];
        $this->a = array_merge($this->a, $b);
    }
}
