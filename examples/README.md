# Examples

Build any example against the JVM and the native compiled binary:

```console
$ javac examples/Hello.java -d /tmp/ex
$ j2r build /tmp/ex/Hello.class -o /tmp/out --release
$ /tmp/out/target/release/j2r_app

$ j2r test examples/Hello.java   # differential check vs the JVM
```
