abstract class GrandParentClassDoesNotDefine {
    public function __construct() {}
}

abstract class ParentClassDefinesVar extends GrandParentClassDoesNotDefine {
    protected string $val;

    public function __construct() {
        $this->val = "hello";
        parent::__construct();
    }
}

final class ChildClass extends ParentClassDefinesVar {
    public function __construct() {
        parent::__construct();
    }
}