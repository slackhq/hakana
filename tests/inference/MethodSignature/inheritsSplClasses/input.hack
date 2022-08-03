namespace App;

use SplObserver;
use SplSubject;

class Observer implements \SplObserver
{
    public function update(SplSubject $subject)
    {
    }
}

class Subject implements \SplSubject
{
    public function attach(SplObserver $observer)
    {
    }

    public function detach(SplObserver $observer)
    {
    }

    public function notify()
    {
    }
}