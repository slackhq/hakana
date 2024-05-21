final class User {}

final class Users extends ArrayIterator<arraykey, User>
{
    public function __construct(User ...$users) {
        parent::__construct($users);
    }
}