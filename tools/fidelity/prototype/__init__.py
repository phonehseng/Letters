"""pdf to docx prototype: every glyph where the pdf puts it, measured through word.

modules, in the order a conversion uses them:
    pdf_fonts       font dictionaries: widths, encodings, tounicode, embedded programs
    pdf_content     content stream interpreter: glyph pen positions, paths, images
    font_rebuild    embedded font programs -> clean truetype fonts word can use
    vectors         simple paths -> drawingml shapes, the rest -> raster of vectors only
    lines           glyphs -> positioned lines and runs
    calibration     frame twips chosen by measurement, cached per word build
    docx_out        the docx package
    convert         the whole pipeline and its report
"""
