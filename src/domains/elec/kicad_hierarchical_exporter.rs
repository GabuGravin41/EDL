/// Phase 1.2 Week 1: KiCad Exporter for Hierarchical Nets
///
/// This module extends the KiCad exporter to generate sub-sheets for
/// hierarchical modules and create hierarchical pins for cross-sheet connections

use std::collections::HashMap;

/// KiCad schematic file representation
#[derive(Clone, Debug)]
pub struct KiCadSchematic {
    /// File version: (6, 0) for KiCad 6+
    pub version: (u32, u32),
    
    /// Generator identifier
    pub generator: String,
    
    /// All sheets in this design
    pub sheets: Vec<Sheet>,
    
    /// Connections/wires within this sheet
    pub connections: Vec<Connection>,
    
    /// Symbols (components) on this sheet
    pub symbols: Vec<Symbol>,
    
    /// Text annotations
    pub texts: Vec<Text>,
}

/// A sheet in the KiCad design (can be main sheet or sub-sheet)
#[derive(Clone, Debug)]
pub struct Sheet {
    /// Sheet filename (relative path)
    /// E.g., "PowerModule.kicad_sch" for /PowerModule/Net nets
    pub filename: String,
    
    /// Sheet name (display name)
    /// E.g., "PowerModule"
    pub name: String,
    
    /// Hierarchical pins on this sheet (for connections to parent)
    /// Maps net name to hierarchical pin
    pub hierarchical_pins: HashMap<String, HierarchicalPin>,
    
    /// UUID for sheet reference
    pub uuid: String,
    
    /// Coordinate on parent sheet
    pub at: (f64, f64),
    
    /// Size of sheet symbol
    pub size: (f64, f64),
    
    /// Is this a sub-sheet? (true) or main sheet (false)
    pub is_subsheet: bool,
}

impl Sheet {
    /// Create a new sub-sheet for a hierarchical module
    pub fn new_subsheet(module_name: &str) -> Self {
        Sheet {
            filename: format!("{}.kicad_sch", module_name.to_lowercase()),
            name: module_name.to_string(),
            hierarchical_pins: HashMap::new(),
            uuid: uuid::Uuid::new_v4().to_string(),
            at: (100.0, 100.0), // Default position on parent sheet
            size: (200.0, 200.0), // Sheet symbol size
            is_subsheet: true,
        }
    }
    
    /// Add a hierarchical pin for cross-sheet connection
    pub fn add_hierarchical_pin(
        &mut self,
        net_name: &str,
        direction: PinDirection,
    ) -> Result<(), ExportError> {
        if self.hierarchical_pins.contains_key(net_name) {
            return Err(ExportError::DuplicateHierarchicalPin(net_name.to_string()));
        }
        
        self.hierarchical_pins.insert(
            net_name.to_string(),
            HierarchicalPin {
                name: net_name.to_string(),
                direction,
                internal_net: net_name.to_string(),
            },
        );
        
        Ok(())
    }
}

/// Direction of a hierarchical pin
#[derive(Clone, Debug, PartialEq)]
pub enum PinDirection {
    Input,
    Output,
    BiDi,  // Bidirectional
}

impl PinDirection {
    /// Convert to KiCad pin_type attribute
    pub fn to_kicad_string(&self) -> &str {
        match self {
            PinDirection::Input => "Input",
            PinDirection::Output => "Output",
            PinDirection::BiDi => "BiDi",
        }
    }
}

/// Hierarchical pin connecting a sub-sheet to parent
#[derive(Clone, Debug)]
pub struct HierarchicalPin {
    pub name: String,
    pub direction: PinDirection,
    pub internal_net: String,
}

/// Connection/wire in the schematic
#[derive(Clone, Debug)]
pub struct Connection {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub net_name: String,
}

/// Symbol (component) in the schematic
#[derive(Clone, Debug)]
pub struct Symbol {
    pub reference: String,  // e.g., "U1", "R1"
    pub value: String,      // e.g., "MP9486", "10k"
    pub at: (f64, f64),
    pub pins: Vec<SymbolPin>,
}

/// Pin on a symbol
#[derive(Clone, Debug)]
pub struct SymbolPin {
    pub number: String,     // e.g., "1", "2", "GND"
    pub net: String,        // Net this pin connects to
}

/// Text annotation in schematic
#[derive(Clone, Debug)]
pub struct Text {
    pub content: String,
    pub at: (f64, f64),
    pub size: f64,
}

/// KiCad exporter for hierarchical nets
pub struct KiCadExporter {
    main_sheet: Sheet,
    subsheets: Vec<Sheet>,
    hierarchical_connections: Vec<HierarchicalConnection>,
}

impl KiCadExporter {
    /// Create new exporter for a design
    pub fn new(project_name: &str) -> Self {
        let mut main_sheet = Sheet {
            filename: format!("{}.kicad_sch", project_name.to_lowercase()),
            name: project_name.to_string(),
            hierarchical_pins: HashMap::new(),
            uuid: uuid::Uuid::new_v4().to_string(),
            at: (0.0, 0.0),
            size: (0.0, 0.0),
            is_subsheet: false,
        };
        
        // Add title block
        main_sheet.texts.push(Text {
            content: format!("Title: {}", project_name),
            at: (100.0, 100.0),
            size: 4.0,
        });
        
        KiCadExporter {
            main_sheet,
            subsheets: Vec::new(),
            hierarchical_connections: Vec::new(),
        }
    }
    
    /// Register a hierarchical module as a sub-sheet
    pub fn register_subsheet(&mut self, module_name: &str) -> Result<(), ExportError> {
        // Check for duplicates
        if self.subsheets.iter().any(|s| s.name == module_name) {
            return Err(ExportError::DuplicateSubsheet(module_name.to_string()));
        }
        
        let sheet = Sheet::new_subsheet(module_name);
        self.subsheets.push(sheet);
        Ok(())
    }
    
    /// Add a hierarchical pin to a sub-sheet
    pub fn add_subsheet_pin(
        &mut self,
        sheet_name: &str,
        net_name: &str,
        direction: PinDirection,
    ) -> Result<(), ExportError> {
        let sheet = self.subsheets.iter_mut()
            .find(|s| s.name == sheet_name)
            .ok_or_else(|| ExportError::UnknownSheet(sheet_name.to_string()))?;
        
        sheet.add_hierarchical_pin(net_name, direction)?;
        Ok(())
    }
    
    /// Connect parent net to sub-sheet hierarchical pin
    pub fn connect_to_subsheet(
        &mut self,
        parent_net: &str,
        sheet_name: &str,
        pin_name: &str,
    ) -> Result<(), ExportError> {
        // Verify sheet and pin exist
        self.subsheets.iter()
            .find(|s| s.name == sheet_name)
            .ok_or_else(|| ExportError::UnknownSheet(sheet_name.to_string()))?;
        
        self.hierarchical_connections.push(HierarchicalConnection {
            parent_net: parent_net.to_string(),
            sheet_name: sheet_name.to_string(),
            pin_name: pin_name.to_string(),
        });
        
        Ok(())
    }
    
    /// Export to KiCad schematic format (.kicad_sch)
    pub fn export_to_kicad_format(&self) -> Result<String, ExportError> {
        let mut output = String::new();
        
        // Header
        output.push_str("(kicad_sch (version 20230121)\n");
        output.push_str("  (generator \"edl_kicad_exporter\")\n\n");
        
        // UUID
        output.push_str(&format!("  (uuid \"{}\")\n\n", self.main_sheet.uuid));
        
        // Paper size
        output.push_str("  (paper \"A4\")\n");
        output.push_str("  (title_block\n");
        output.push_str(&format!("    (title \"{}\")\n", self.main_sheet.name));
        output.push_str("  )\n\n");
        
        // Export sub-sheets
        for sheet in &self.subsheets {
            output.push_str(&self.format_sheet_reference(sheet)?);
        }
        
        // Export connections
        for conn in &self.hierarchical_connections {
            output.push_str(&self.format_hierarchical_connection(conn)?);
        }
        
        output.push_str(")\n");
        
        Ok(output)
    }
    
    /// Format a sheet reference for KiCad
    fn format_sheet_reference(&self, sheet: &Sheet) -> Result<String, ExportError> {
        let mut output = String::new();
        
        output.push_str("  (sheet_instance\n");
        output.push_str(&format!("    (at {} {})\n", sheet.at.0, sheet.at.1));
        output.push_str(&format!("    (size {} {})\n", sheet.size.0, sheet.size.1));
        output.push_str(&format!("    (uuid \"{}\")\n", sheet.uuid));
        
        // Sheet properties
        output.push_str("    (property \"Sheetname\" ");
        output.push_str(&format!("\"{}\"", sheet.name));
        output.push_str(")\n");
        
        output.push_str("    (property \"Sheetfile\" ");
        output.push_str(&format!("\"{}\"", sheet.filename));
        output.push_str(")\n");
        
        // Hierarchical pins (sub-sheet connections)
        for (net_name, pin) in &sheet.hierarchical_pins {
            output.push_str(&format!("    (hierarchical_pin \"{}\" \"{}\")\n",
                net_name,
                pin.direction.to_kicad_string()
            ));
        }
        
        output.push_str("  )\n\n");
        
        Ok(output)
    }
    
    /// Format a hierarchical connection in KiCad
    fn format_hierarchical_connection(
        &self,
        conn: &HierarchicalConnection,
    ) -> Result<String, ExportError> {
        let mut output = String::new();
        
        output.push_str("  (hierarchical_connection\n");
        output.push_str(&format!("    (parent_net \"{}\")\n", conn.parent_net));
        output.push_str(&format!("    (sheet \"{}\")\n", conn.sheet_name));
        output.push_str(&format!("    (pin \"{}\")\n", conn.pin_name));
        output.push_str("  )\n");
        
        Ok(output)
    }
    
    /// Generate module files (each sub-sheet gets its own .kicad_sch)
    pub fn generate_module_files(&self) -> Result<Vec<ModuleFile>, ExportError> {
        let mut files = vec![];
        
        for sheet in &self.subsheets {
            let content = self.generate_module_file_content(sheet)?;
            files.push(ModuleFile {
                filename: sheet.filename.clone(),
                content,
            });
        }
        
        Ok(files)
    }
    
    /// Generate content for a module's .kicad_sch file
    fn generate_module_file_content(&self, sheet: &Sheet) -> Result<String, ExportError> {
        let mut output = String::new();
        
        output.push_str(&format!("(kicad_sch (version 20230121)\n"));
        output.push_str(&format!("  (generator \"edl_kicad_exporter\")\n\n"));
        output.push_str(&format!("  (uuid \"{}\")\n\n", sheet.uuid));
        output.push_str(&format!("  (paper \"A4\")\n"));
        output.push_str(&format!("  (title_block\n"));
        output.push_str(&format!("    (title \"{}\")\n", sheet.name));
        output.push_str(&format!("  )\n\n"));
        
        // Hierarchical pins as ports
        for (net_name, pin) in &sheet.hierarchical_pins {
            output.push_str(&format!("  (hierarchical_pin_declaration\n"));
            output.push_str(&format!("    (name \"{}\")\n", net_name));
            output.push_str(&format!("    (type \"{}\")\n", pin.direction.to_kicad_string()));
            output.push_str(&format!("  )\n"));
        }
        
        output.push_str(")\n");
        
        Ok(output)
    }
}

impl Default for KiCadExporter {
    fn default() -> Self {
        Self::new("design")
    }
}

/// Hierarchical connection between parent and sub-sheet
#[derive(Clone, Debug)]
pub struct HierarchicalConnection {
    pub parent_net: String,
    pub sheet_name: String,
    pub pin_name: String,
}

/// Generated KiCad module file
pub struct ModuleFile {
    pub filename: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub enum ExportError {
    DuplicateSubsheet(String),
    DuplicateHierarchicalPin(String),
    UnknownSheet(String),
    IOError(String),
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sheet_creation() {
        let sheet = Sheet::new_subsheet("PowerModule");
        assert_eq!(sheet.name, "PowerModule");
        assert!(sheet.filename.contains("powermodule"));
        assert!(sheet.is_subsheet);
    }
    
    #[test]
    fn test_exporter_creation() {
        let exporter = KiCadExporter::new("MyBoard");
        assert_eq!(exporter.main_sheet.name, "MyBoard");
        assert!(!exporter.main_sheet.is_subsheet);
    }
    
    #[test]
    fn test_register_subsheet() {
        let mut exporter = KiCadExporter::new("MainBoard");
        exporter.register_subsheet("PowerModule").unwrap();
        
        assert_eq!(exporter.subsheets.len(), 1);
        assert_eq!(exporter.subsheets[0].name, "PowerModule");
    }
    
    #[test]
    fn test_add_hierarchical_pin() {
        let mut exporter = KiCadExporter::new("MainBoard");
        exporter.register_subsheet("PowerModule").unwrap();
        
        exporter.add_subsheet_pin(
            "PowerModule",
            "VOUT",
            PinDirection::Output,
        ).unwrap();
        
        let sheet = &exporter.subsheets[0];
        assert!(sheet.hierarchical_pins.contains_key("VOUT"));
    }
    
    #[test]
    fn test_connect_subsheet() {
        let mut exporter = KiCadExporter::new("MainBoard");
        exporter.register_subsheet("PowerModule").unwrap();
        exporter.add_subsheet_pin(
            "PowerModule",
            "VOUT",
            PinDirection::Output,
        ).unwrap();
        
        exporter.connect_to_subsheet(
            "VCC",
            "PowerModule",
            "VOUT",
        ).unwrap();
        
        assert_eq!(exporter.hierarchical_connections.len(), 1);
    }
    
    #[test]
    fn test_export_to_kicad_format() {
        let mut exporter = KiCadExporter::new("PowerBoard");
        exporter.register_subsheet("PowerModule").unwrap();
        exporter.add_subsheet_pin(
            "PowerModule",
            "VOUT",
            PinDirection::Output,
        ).unwrap();
        
        let result = exporter.export_to_kicad_format().unwrap();
        
        assert!(result.contains("kicad_sch"));
        assert!(result.contains("PowerModule"));
        assert!(result.contains("VOUT"));
    }
    
    #[test]
    fn test_generate_module_files() {
        let mut exporter = KiCadExporter::new("MainBoard");
        exporter.register_subsheet("PowerModule").unwrap();
        exporter.register_subsheet("SignalProc").unwrap();
        
        let files = exporter.generate_module_files().unwrap();
        assert_eq!(files.len(), 2);
        
        assert!(files[0].filename.contains("powermodule"));
        assert!(files[1].filename.contains("signalproc"));
    }
    
    #[test]
    fn test_duplicate_subsheet_error() {
        let mut exporter = KiCadExporter::new("MainBoard");
        exporter.register_subsheet("PowerModule").unwrap();
        
        let result = exporter.register_subsheet("PowerModule");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_pin_direction_to_kicad() {
        assert_eq!(PinDirection::Input.to_kicad_string(), "Input");
        assert_eq!(PinDirection::Output.to_kicad_string(), "Output");
        assert_eq!(PinDirection::BiDi.to_kicad_string(), "BiDi");
    }
    
    #[test]
    fn test_multisheet_power_distribution() {
        let mut exporter = KiCadExporter::new("PowerBoard");
        
        // Create hierarchy: Primary -> Secondary stages
        exporter.register_subsheet("PrimaryStage").unwrap();
        exporter.register_subsheet("SecondaryStage").unwrap();
        
        // Primary stage outputs 3.3V
        exporter.add_subsheet_pin(
            "PrimaryStage",
            "VOUT",
            PinDirection::Output,
        ).unwrap();
        
        // Secondary stage takes 3.3V input
        exporter.add_subsheet_pin(
            "SecondaryStage",
            "VIN",
            PinDirection::Input,
        ).unwrap();
        
        // Connect primary output to secondary input
        exporter.connect_to_subsheet(
            "/PrimaryStage/VOUT",
            "SecondaryStage",
            "VIN",
        ).unwrap();
        
        let result = exporter.export_to_kicad_format().unwrap();
        
        assert!(result.contains("PrimaryStage"));
        assert!(result.contains("SecondaryStage"));
        assert!(result.contains("VOUT"));
        assert!(result.contains("VIN"));
    }
}
