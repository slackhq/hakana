final class User {
    public string $id;

    public function __construct(string $userId) {
        $this->id = $userId;
    }

    public function setId(string $userId) : void {
        $this->id = $userId;
    }
}

function echoId(User $u2) : void {
    echo $u2->id;
}

$u = new User("5");
echoId($u);
$u->setId($_GET["user_id"]);