<<__Sealed(Child::class)>>
class Base {
    <<Hakana\MustUse>>
    public function __construct() {}
}

final class Child extends Base {
    public function __construct() {
        parent::__construct();
    }
}
