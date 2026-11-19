package minimaljsonentry;

/**
 * Upstream package-private static nested class of JsonObject, now a
 * package-private top-level carrier in its own sourcepath-discoverable
 * file (scanaislop round-4 reshape, slice-k — javac's -sourcepath
 * resolution finds top-level classes only by their own file name); the
 * hash-index bytes and semantics are unchanged.
 */
class HashIndexTable {

  private final byte[] hashTable = new byte[32]; // must be a power of two

  HashIndexTable() {
  }

  HashIndexTable(HashIndexTable original) {
    System.arraycopy(original.hashTable, 0, hashTable, 0, hashTable.length);
  }

  void add(String name, int index) {
    int slot = hashSlotFor(name);
    if (index < 0xff) {
      hashTable[slot] = (byte)(index + 1);
    } else {
      hashTable[slot] = 0;
    }
  }

  void remove(int index) {
    for (int i = 0; i < hashTable.length; i++) {
      if ((hashTable[i] & 0xff) == index + 1) {
        hashTable[i] = 0;
      } else if ((hashTable[i] & 0xff) > index + 1) {
        hashTable[i]--;
      }
    }
  }

  int get(Object name) {
    int slot = hashSlotFor(name);
    // subtract 1, 0 stands for empty
    return (hashTable[slot] & 0xff) - 1;
  }

  private int hashSlotFor(Object element) {
    return element.hashCode() & hashTable.length - 1;
  }

}
