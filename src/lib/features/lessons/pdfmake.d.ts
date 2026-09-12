/**
 * The parts of pdfmake 0.3's browser build the desk uses. The package ships
 * no types for this build; the shapes below are the documented ones.
 */
declare module 'pdfmake/build/pdfmake' {
  export interface PdfFontFamily {
    normal: string;
    bold: string;
    italics: string;
    bolditalics: string;
  }
  export interface PdfOutput {
    getBuffer(): Promise<Uint8Array>;
    getBlob(): Promise<Blob>;
  }
  export interface PdfMake {
    addVirtualFileSystem(vfs: Record<string, string>): void;
    setFonts(fonts: Record<string, PdfFontFamily>): void;
    setUrlAccessPolicy(policy: (url: string) => boolean): void;
    createPdf(definition: Record<string, unknown>): PdfOutput;
  }
  const pdfmake: PdfMake;
  export default pdfmake;
}
