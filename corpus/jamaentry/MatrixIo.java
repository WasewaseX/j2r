package jamaentry;

/*
 * Derived from JAMA 1.0.3 (NIST public domain); part of the j2r
 * real-world corpus bench entry (the corpus/JamaBench driver family).
 * Round-3: MatrixIo holds the construction and I/O group (constructWithCopy/random/identity/read/print and the packed-copy exporters) of the JAMA Matrix, split from the
 * single-file Matrix source per the owner's scanaislop complexity
 * order (file above 441 lines). The moved bodies are the upstream
 * bodies with the receiver made explicit where the file split
 * requires it (the receiver is named mx so the bare upstream field
 * reads A/m/n become mx.A/mx.m/mx.n) - behavior byte-identical,
 * pinned by the javac+java stdout proof and the j2r three-channel
 * gate. Hand-maintained after this generation step.
 */

import java.text.NumberFormat;
import java.text.DecimalFormat;
import java.text.DecimalFormatSymbols;
import java.text.FieldPosition;
import java.util.Locale;
import java.io.PrintWriter;
import java.io.BufferedReader;
import java.io.StreamTokenizer;

final class MatrixIo {

   /** Construct a matrix from a copy of a 2-D array.
   @param A    Two-dimensional array of doubles.
   @exception  IllegalArgumentException All rows must have the same length
   */

   public static Matrix constructWithCopy(double[][] A) {
      int m = A.length;
      int n = A[0].length;
      Matrix X = new Matrix(m,n);
      double[][] C = X.getArray();
      for (int i = 0; i < m; i++) {
         if (A[i].length != n) {
            throw new IllegalArgumentException
               ("All rows must have the same length.");
         }
         for (int j = 0; j < n; j++) {
            C[i][j] = A[i][j];
         }
      }
      return X;
   }

   /** Make a one-dimensional column packed copy of the internal array.
   @return     Matrix elements packed in a one-dimensional array by columns.
   */

   static double[] getColumnPackedCopy (Matrix mx) {
      double[] vals = new double[mx.m*mx.n];
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            vals[i+j*mx.m] = mx.A[i][j];
         }
      }
      return vals;
   }

   /** Make a one-dimensional row packed copy of the internal array.
   @return     Matrix elements packed in a one-dimensional array by rows.
   */

   static double[] getRowPackedCopy (Matrix mx) {
      double[] vals = new double[mx.m*mx.n];
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            vals[i*mx.n+j] = mx.A[i][j];
         }
      }
      return vals;
   }

   /** Generate matrix with random elements
   @param m    Number of rows.
   @param n    Number of colums.
   @return     An m-by-n matrix with uniformly distributed random elements.
   */

   public static Matrix random (int m, int n) {
      Matrix A = new Matrix(m,n);
      double[][] X = A.getArray();
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            X[i][j] = Math.random();
         }
      }
      return A;
   }

   /** Generate identity matrix
   @param m    Number of rows.
   @param n    Number of colums.
   @return     An m-by-n matrix with ones on the diagonal and zeros elsewhere.
   */

   public static Matrix identity (int m, int n) {
      Matrix A = new Matrix(m,n);
      double[][] X = A.getArray();
      for (int i = 0; i < m; i++) {
         for (int j = 0; j < n; j++) {
            X[i][j] = (i == j ? 1.0 : 0.0);
         }
      }
      return A;
   }


   /** Print the matrix to stdout.   Line the elements up in columns
     * with a Fortran-like 'Fw.d' style format.
   @param w    Column width.
   @param d    Number of digits after the decimal.
   */

   static void print (Matrix mx, int w, int d) {
      print(mx, new PrintWriter(System.out,true),w,d); }

   /** Print the matrix to the output stream.   Line the elements up in
     * columns with a Fortran-like 'Fw.d' style format.
   @param output Output stream.
   @param w      Column width.
   @param d      Number of digits after the decimal.
   */

   static void print (Matrix mx, PrintWriter output, int w, int d) {
      DecimalFormat format = new DecimalFormat();
      format.setDecimalFormatSymbols(new DecimalFormatSymbols(Locale.US));
      format.setMinimumIntegerDigits(1);
      format.setMaximumFractionDigits(d);
      format.setMinimumFractionDigits(d);
      format.setGroupingUsed(false);
      print(mx, output,format,w+2);
   }

   /** Print the matrix to stdout.  Line the elements up in columns.
     * Use the format object, and right justify within columns of width
     * characters.
     * Note that is the matrix is to be read back in, you probably will want
     * to use a NumberFormat that is set to US Locale.
   @param format A  Formatting object for individual elements.
   @param width     Field width for each column.
   @see java.text.DecimalFormat#setDecimalFormatSymbols
   */

   static void print (Matrix mx, NumberFormat format, int width) {
      print(mx, new PrintWriter(System.out,true),format,width); }

   // DecimalFormat is a little disappointing coming from Fortran or C's printf.
   // Since it doesn't pad on the left, the elements will come out different
   // widths.  Consequently, we'll pass the desired column width in as an
   // argument and do the extra padding ourselves.

   /** Print the matrix to the output stream.  Line the elements up in columns.
     * Use the format object, and right justify within columns of width
     * characters.
     * Note that is the matrix is to be read back in, you probably will want
     * to use a NumberFormat that is set to US Locale.
   @param output the output stream.
   @param format A formatting object to format the matrix elements 
   @param width  Column width.
   @see java.text.DecimalFormat#setDecimalFormatSymbols
   */

   static void print (Matrix mx, PrintWriter output, NumberFormat format, int width) {
      output.println();  // start on new line.
      for (int i = 0; i < mx.m; i++) {
         for (int j = 0; j < mx.n; j++) {
            String s = format.format(mx.A[i][j]); // format the number
            int padding = Math.max(1,width-s.length()); // At _least_ 1 space
            for (int k = 0; k < padding; k++)
               output.print(' ');
            output.print(s);
         }
         output.println();
      }
      output.println();   // end with blank line.
   }

   /** Read a matrix from a stream.  The format is the same the print method,
     * so printed matrices can be read back in (provided they were printed using
     * US Locale).  Elements are separated by
     * whitespace, all the elements for each row appear on a single line,
     * the last row is followed by a blank line.
   @param input the input stream.
   */

   public static Matrix read (BufferedReader input) throws java.io.IOException {
      StreamTokenizer tokenizer= new StreamTokenizer(input);

      // Although StreamTokenizer will parse numbers, it doesn't recognize
      // scientific notation (E or D); however, Double.valueOf does.
      // The strategy here is to disable StreamTokenizer's number parsing.
      // We'll only get whitespace delimited words, EOL's and EOF's.
      // These words should all be numbers, for Double.valueOf to parse.

      tokenizer.resetSyntax();
      tokenizer.wordChars(0,255);
      tokenizer.whitespaceChars(0, ' ');
      tokenizer.eolIsSignificant(true);
      java.util.Vector<Double> vD = new java.util.Vector<Double>();

      // Ignore initial empty lines
      while (tokenizer.nextToken() == StreamTokenizer.TT_EOL);
      if (tokenizer.ttype == StreamTokenizer.TT_EOF)
	throw new java.io.IOException("Unexpected EOF on matrix read.");
      do {
         vD.addElement(Double.valueOf(tokenizer.sval)); // Read & store 1st row.
      } while (tokenizer.nextToken() == StreamTokenizer.TT_WORD);

      int n = vD.size();  // Now we've got the number of columns!
      double row[] = new double[n];
      for (int j=0; j<n; j++)  // extract the elements of the 1st row.
         row[j]=vD.elementAt(j).doubleValue();
      java.util.Vector<double[]> v = new java.util.Vector<double[]>();
      v.addElement(row);  // Start storing rows instead of columns.
      while (tokenizer.nextToken() == StreamTokenizer.TT_WORD) {
         // While non-empty lines
         v.addElement(row = new double[n]);
         int j = 0;
         do {
            if (j >= n) throw new java.io.IOException
               ("Row " + v.size() + " is too long.");
            row[j++] = Double.valueOf(tokenizer.sval).doubleValue();
         } while (tokenizer.nextToken() == StreamTokenizer.TT_WORD);
         if (j < n) throw new java.io.IOException
            ("Row " + v.size() + " is too short.");
      }
      int m = v.size();  // Now we've got the number of rows.
      double[][] A = new double[m][];
      v.copyInto(A);  // copy the rows out of the vector
      return new Matrix(A);
   }


/* ------------------------
   Private Methods
 * ------------------------ */

  private MatrixIo() {}
}
